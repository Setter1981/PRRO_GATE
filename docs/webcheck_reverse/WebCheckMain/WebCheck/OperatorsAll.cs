using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class OperatorsAll
{
	private string[,] Op;

	private int OpS;

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

	public OperatorsAll()
	{
		Op = new string[5, 1];
		OpS = 0;
		Op = new string[5, checked(OpS + 1)];
		LoadOperatorsAll();
	}

	private void LoadOperatorsAll()
	{
		checked
		{
			try
			{
				string connection = All.A.Connection;
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM OPERATORS";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					OpS++;
					ref string[,] op = ref Op;
					op = (string[,])Utils.CopyArray((Array)op, (Array)new string[5, OpS + 1]);
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

	internal bool OperatorDefault(int idN)
	{
		//IL_0142: Unknown result type (might be due to invalid IL or missing references)
		if (OpS < 2)
		{
			return false;
		}
		TypErrOperKeyPassNameINN operInfa = default(TypErrOperKeyPassNameINN);
		operInfa.errCode = 0;
		operInfa.errStr = "";
		operInfa.Name = Op[1, 1];
		operInfa.KeyFile = Op[2, 1];
		operInfa.Pass = Op[3, 1];
		operInfa.INN = Op[4, 1];
		int num = IdOperatorToNm(idN);
		if (num == 0)
		{
			return false;
		}
		TypErrOperKeyPassNameINN operInfa2 = default(TypErrOperKeyPassNameINN);
		operInfa2.errCode = 0;
		operInfa2.errStr = "";
		operInfa2.Name = Op[1, num];
		operInfa2.KeyFile = Op[2, num];
		operInfa2.Pass = Op[3, num];
		operInfa2.INN = Op[4, num];
		if (Operators.CompareString(operInfa2.INN[0].ToString().ToUpper(), "R", false) == 0)
		{
			if ((All.A.TypWork < 2019) | (All.A.TypWork > 2020))
			{
				Interaction.MsgBox((object)"Резервний ключ не може бути оператором за замовчанням", (MsgBoxStyle)48, (object)"Увага");
			}
			return false;
		}
		UPDATE("1", operInfa2);
		UPDATE(idN.ToString(), operInfa);
		All.Lg.SaveTextToLog("OperatorDefault", "Оператор ИНН " + operInfa2.INN + " назначен по умолчанию");
		return true;
	}

	internal int IdOperatorToNm(int idOp)
	{
		if (OpS < 1)
		{
			return 0;
		}
		int opS = OpS;
		for (int i = 1; i <= opS; i = checked(i + 1))
		{
			if (Operators.CompareString(Op[0, i], idOp.ToString(), false) == 0)
			{
				return i;
			}
		}
		return 0;
	}

	internal TypErr UPDATE(string ID, TypErrOperKeyPassNameINN OperInfa)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE OPERATORS SET OPERATORNAME='" + OperInfa.Name + "', KEYPATH='" + OperInfa.KeyFile + "', KEYPASS='" + OperInfa.Pass + "', INN='" + OperInfa.INN + "' WHERE ID='" + ID + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errStr = "Ошибка при попытке изменить запись в таблице OPERATORS.";
				result.errCode = 96;
				ProjectData.ClearProjectError();
				goto IL_00fc;
			}
			break;
			IL_00fc:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	public int CountOperators(string innS)
	{
		string text = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM OPERATORS WHERE INN = '" + innS + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				text = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(text);
	}
}
