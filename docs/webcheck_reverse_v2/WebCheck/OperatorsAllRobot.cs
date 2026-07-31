using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class OperatorsAllRobot
{
	private string[,] Op;

	private int OpS;

	private string ConnectionS;

	private string FN;

	public int Operators => OpS;

	public string Seller
	{
		get
		{
			if (x < 0 || x > 4)
			{
				return "";
			}
			if ((y < 1) | (y > OpS))
			{
				return "";
			}
			return Op[x, y];
		}
	}

	public OperatorsAllRobot(string ConnectionT, string FNs)
	{
		Op = new string[5, 1];
		FN = FNs;
		ConnectionS = ConnectionT;
		OpS = 0;
		Op = new string[5, checked(OpS + 1)];
		LoadOperatorsAll();
		Application.DoEvents();
		EndKeyToINI();
	}

	private void LoadOperatorsAll()
	{
		checked
		{
			try
			{
				string connectionS = ConnectionS;
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connectionS;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM OPERATORS";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray(op, new string[5, OpS + 1]);
					Op[0, OpS] = sQLiteDataReader[0].ToString();
					Op[1, OpS] = sQLiteDataReader[1].ToString();
					Op[2, OpS] = sQLiteDataReader[2].ToString();
					Op[3, OpS] = sQLiteDataReader[3].ToString();
					Op[4, OpS] = sQLiteDataReader[4].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				OpS = 0;
				Op = new string[5, OpS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}

	public int CountOperators(string innS)
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = ConnectionS;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM OPERATORS WHERE INN = '" + innS + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}

	internal void EndKeyToINI()
	{
		if (OpS < 1)
		{
			return;
		}
		string text = DateTime.Now.Day.ToString();
		IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\dat.ini");
		int opS = OpS;
		for (int i = 1; i <= opS; i = checked(i + 1))
		{
			if (Microsoft.VisualBasic.CompilerServices.Operators.CompareString(iniHGB.GetString(Op[4, i], "Updated").Trim(), text.Trim(), TextCompare: false) != 0)
			{
				Coding coding = new Coding();
				TypErrStrCert typErrStrCert = All.SF.Cert(Op[2, i], coding.DeCod(Op[3, i]));
				if (typErrStrCert.errCode == 0)
				{
					iniHGB.WriteString(Op[4, i], "StartKey", typErrStrCert.ReturnStart);
					iniHGB.WriteString(Op[4, i], "EndKey", typErrStrCert.ReturnEnd);
					iniHGB.WriteString(Op[4, i], "Updated", text);
					iniHGB.WriteString(Op[4, i], "Serial", typErrStrCert.ReturnSerial);
					iniHGB.WriteString(Op[4, i], "Issuer", typErrStrCert.ReturnIssuer);
				}
				else
				{
					iniHGB.WriteString(Op[4, i], "Updated", text);
				}
			}
		}
	}
}
