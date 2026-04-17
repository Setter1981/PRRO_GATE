using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class ShiftAll
{
	private string[,] Shift;

	private int ShiftS;

	public int ShiftsYear => ShiftS;

	public string Seller
	{
		get
		{
			if (x < 0 || x > 1)
			{
				return "";
			}
			if ((y < 1) | (y > ShiftS))
			{
				return "";
			}
			return Shift[x, y];
		}
	}

	public ShiftAll(string eYear)
	{
		Shift = new string[2, 1];
		ShiftS = 0;
		Shift = new string[2, checked(ShiftS + 1)];
		LoadShiftsAll(eYear);
	}

	private void LoadShiftsAll(string eYear)
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
				sQLiteCommand.CommandText = "Select id, DATEEND from SHIFTS where date(DATEEND, 'start of year')=date('" + eYear + "-01-01', 'start of year')";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					ShiftS++;
					ref string[,] shift = ref Shift;
					shift = (string[,])Utils.CopyArray((Array)shift, (Array)new string[2, ShiftS + 1]);
					Shift[0, ShiftS] = sQLiteDataReader[0].ToString();
					Shift[1, ShiftS] = sQLiteDataReader[1].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ShiftS = 0;
				Shift = new string[2, ShiftS + 1];
				ProjectData.ClearProjectError();
			}
		}
	}
}
