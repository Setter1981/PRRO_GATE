using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class Form5 : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ADDoperator")]
	private Button _ADDoperator;

	[CompilerGenerated]
	[AccessedThroughProperty("InnT")]
	private TextBox _InnT;

	[CompilerGenerated]
	[AccessedThroughProperty("TinT")]
	private TextBox _TinT;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ADDoperator
	{
		[CompilerGenerated]
		get
		{
			return _ADDoperator;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ADDoperator_Click;
			Button aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click -= eventHandler;
			}
			_ADDoperator = value;
			aDDoperator = _ADDoperator;
			if (aDDoperator != null)
			{
				((Control)aDDoperator).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("OrgT")]
	internal virtual TextBox OrgT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox InnT
	{
		[CompilerGenerated]
		get
		{
			return _InnT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = InnT_TextChanged;
			TextBox innT = _InnT;
			if (innT != null)
			{
				((Control)innT).TextChanged -= eventHandler;
			}
			_InnT = value;
			innT = _InnT;
			if (innT != null)
			{
				((Control)innT).TextChanged += eventHandler;
			}
		}
	}

	internal virtual TextBox TinT
	{
		[CompilerGenerated]
		get
		{
			return _TinT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TinT_TextChanged;
			TextBox tinT = _TinT;
			if (tinT != null)
			{
				((Control)tinT).TextChanged -= eventHandler;
			}
			_TinT = value;
			tinT = _TinT;
			if (tinT != null)
			{
				((Control)tinT).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("FnT")]
	internal virtual TextBox FnT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PonT")]
	internal virtual TextBox PonT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PntT")]
	internal virtual TextBox PntT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ID")]
	internal virtual DataGridViewTextBoxColumn ID
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("FNn")]
	internal virtual DataGridViewTextBoxColumn FNn
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TIN")]
	internal virtual DataGridViewTextBoxColumn TIN
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("INN")]
	internal virtual DataGridViewTextBoxColumn INN
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("POINTNAME")]
	internal virtual DataGridViewTextBoxColumn POINTNAME
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ORGNAME")]
	internal virtual DataGridViewTextBoxColumn ORGNAME
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("POINTADDR")]
	internal virtual DataGridViewTextBoxColumn POINTADDR
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public Form5()
	{
		((Form)this).Load += Form5_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_03c3: Unknown result type (might be due to invalid IL or missing references)
		//IL_03cd: Expected O, but got Unknown
		//IL_0449: Unknown result type (might be due to invalid IL or missing references)
		//IL_0453: Expected O, but got Unknown
		//IL_04cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_04d6: Expected O, but got Unknown
		//IL_055b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0565: Expected O, but got Unknown
		//IL_05db: Unknown result type (might be due to invalid IL or missing references)
		//IL_05e5: Expected O, but got Unknown
		//IL_0661: Unknown result type (might be due to invalid IL or missing references)
		//IL_066b: Expected O, but got Unknown
		//IL_0789: Unknown result type (might be due to invalid IL or missing references)
		//IL_0793: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(Form5));
		DG = new DataGridView();
		ID = new DataGridViewTextBoxColumn();
		FNn = new DataGridViewTextBoxColumn();
		TIN = new DataGridViewTextBoxColumn();
		INN = new DataGridViewTextBoxColumn();
		POINTNAME = new DataGridViewTextBoxColumn();
		ORGNAME = new DataGridViewTextBoxColumn();
		POINTADDR = new DataGridViewTextBoxColumn();
		ADDoperator = new Button();
		OrgT = new TextBox();
		InnT = new TextBox();
		TinT = new TextBox();
		FnT = new TextBox();
		PonT = new TextBox();
		PntT = new TextBox();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[7]
		{
			(DataGridViewColumn)ID,
			(DataGridViewColumn)FNn,
			(DataGridViewColumn)TIN,
			(DataGridViewColumn)INN,
			(DataGridViewColumn)POINTNAME,
			(DataGridViewColumn)ORGNAME,
			(DataGridViewColumn)POINTADDR
		});
		((Control)DG).Location = new Point(0, 0);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		((Control)DG).Size = new Size(977, 340);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)ID).HeaderText = "ID";
		((DataGridViewColumn)ID).Name = "ID";
		((DataGridViewColumn)ID).ReadOnly = true;
		((DataGridViewColumn)FNn).HeaderText = "FN";
		((DataGridViewColumn)FNn).Name = "FNn";
		((DataGridViewColumn)FNn).ReadOnly = true;
		((DataGridViewColumn)FNn).Width = 108;
		((DataGridViewColumn)TIN).HeaderText = "TIN";
		((DataGridViewColumn)TIN).Name = "TIN";
		((DataGridViewColumn)TIN).ReadOnly = true;
		((DataGridViewColumn)TIN).Width = 108;
		((DataGridViewColumn)INN).HeaderText = "INN";
		((DataGridViewColumn)INN).Name = "INN";
		((DataGridViewColumn)INN).ReadOnly = true;
		((DataGridViewColumn)INN).Width = 108;
		((DataGridViewColumn)POINTNAME).HeaderText = "POINTNAME";
		((DataGridViewColumn)POINTNAME).Name = "POINTNAME";
		((DataGridViewColumn)POINTNAME).ReadOnly = true;
		((DataGridViewColumn)POINTNAME).Width = 150;
		((DataGridViewColumn)ORGNAME).HeaderText = "ORGNAME";
		((DataGridViewColumn)ORGNAME).Name = "ORGNAME";
		((DataGridViewColumn)ORGNAME).ReadOnly = true;
		((DataGridViewColumn)ORGNAME).Width = 150;
		((DataGridViewColumn)POINTADDR).HeaderText = "POINTADDR";
		((DataGridViewColumn)POINTADDR).Name = "POINTADDR";
		((DataGridViewColumn)POINTADDR).ReadOnly = true;
		((DataGridViewColumn)POINTADDR).Width = 150;
		((Control)ADDoperator).Anchor = (AnchorStyles)6;
		((Control)ADDoperator).Location = new Point(801, 376);
		((Control)ADDoperator).Name = "ADDoperator";
		((Control)ADDoperator).Size = new Size(165, 33);
		((Control)ADDoperator).TabIndex = 7;
		((ButtonBase)ADDoperator).Text = "Добавить";
		((ButtonBase)ADDoperator).UseVisualStyleBackColor = true;
		((Control)OrgT).Anchor = (AnchorStyles)6;
		((Control)OrgT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OrgT).Location = new Point(589, 346);
		((Control)OrgT).Name = "OrgT";
		((Control)OrgT).Size = new Size(181, 24);
		((Control)OrgT).TabIndex = 5;
		OrgT.TextAlign = (HorizontalAlignment)2;
		((Control)InnT).Anchor = (AnchorStyles)6;
		((Control)InnT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)InnT).Location = new Point(272, 346);
		((Control)InnT).Name = "InnT";
		((Control)InnT).Size = new Size(124, 24);
		((Control)InnT).TabIndex = 3;
		InnT.TextAlign = (HorizontalAlignment)2;
		((Control)TinT).Anchor = (AnchorStyles)6;
		((Control)TinT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TinT).Location = new Point(142, 346);
		((Control)TinT).Name = "TinT";
		((Control)TinT).Size = new Size(124, 24);
		((Control)TinT).TabIndex = 2;
		TinT.TextAlign = (HorizontalAlignment)2;
		((Control)FnT).Anchor = (AnchorStyles)6;
		((Control)FnT).Enabled = false;
		((Control)FnT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FnT).Location = new Point(12, 346);
		((Control)FnT).Name = "FnT";
		((Control)FnT).Size = new Size(124, 24);
		((Control)FnT).TabIndex = 1;
		FnT.TextAlign = (HorizontalAlignment)2;
		((Control)PonT).Anchor = (AnchorStyles)6;
		((Control)PonT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PonT).Location = new Point(402, 346);
		((Control)PonT).Name = "PonT";
		((Control)PonT).Size = new Size(181, 24);
		((Control)PonT).TabIndex = 4;
		PonT.TextAlign = (HorizontalAlignment)2;
		((Control)PntT).Anchor = (AnchorStyles)6;
		((Control)PntT).Font = new Font("Microsoft Sans Serif", 11.25f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PntT).Location = new Point(776, 346);
		((Control)PntT).Name = "PntT";
		((Control)PntT).Size = new Size(190, 24);
		((Control)PntT).TabIndex = 6;
		PntT.TextAlign = (HorizontalAlignment)2;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(6f, 13f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(978, 421);
		((Control)this).Controls.Add((Control)(object)PntT);
		((Control)this).Controls.Add((Control)(object)PonT);
		((Control)this).Controls.Add((Control)(object)ADDoperator);
		((Control)this).Controls.Add((Control)(object)OrgT);
		((Control)this).Controls.Add((Control)(object)InnT);
		((Control)this).Controls.Add((Control)(object)TinT);
		((Control)this).Controls.Add((Control)(object)FnT);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Control)this).Name = "Form5";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "TAXOBJECTS";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void Form5_Load(object sender, EventArgs e)
	{
		((Form)this).Text = "TAXOBJECTS";
		FnT.Text = WebCheck.All.FN;
		LoadOperators();
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = connectionString;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "Select * FROM TAXOBJECTS";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				while (sQLiteDataReader.Read())
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[0]);
					DG[1, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[1]);
					DG[2, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[2]);
					DG[3, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[3]);
					DG[4, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[4]);
					DG[5, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[5]);
					DG[6, DG.RowCount - 1].Value = RuntimeHelpers.GetObjectValue(sQLiteDataReader[6]);
				}
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				((Form)this).Text = "TAXOBJECTS " + WebCheck.All.l.MaxID("TAXOBJECTS");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ADDoperator_Click(object sender, EventArgs e)
	{
		if (!Versioned.IsNumeric((object)TinT.Text))
		{
			TinT.Text = "";
			((Control)TinT).Focus();
			return;
		}
		if (!Versioned.IsNumeric((object)InnT.Text))
		{
			InnT.Text = "";
			((Control)InnT).Focus();
			return;
		}
		if (Operators.CompareString(PonT.Text.Trim(), "", false) == 0)
		{
			((Control)PonT).Focus();
			return;
		}
		if (Operators.CompareString(OrgT.Text.Trim(), "", false) == 0)
		{
			((Control)OrgT).Focus();
			return;
		}
		if (Operators.CompareString(PntT.Text.Trim(), "", false) == 0)
		{
			((Control)PntT).Focus();
			return;
		}
		string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = connectionString;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO TAXOBJECTS (FN, TIN, INN, POINTNAME, ORGNAME, POINTADDR ) VALUES ('" + FnT.Text + "','" + TinT.Text + "','" + InnT.Text + "','" + PonT.Text + "','" + OrgT.Text + "','" + PntT.Text + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		TinT.Text = "";
		InnT.Text = "";
		PonT.Text = "";
		OrgT.Text = "";
		PntT.Text = "";
		LoadOperators();
	}

	private void TinT_TextChanged(object sender, EventArgs e)
	{
		if (TinT.Text.Length > 10)
		{
			TinT.Text = Strings.Mid(TinT.Text, 1, 10);
		}
	}

	private void InnT_TextChanged(object sender, EventArgs e)
	{
		if (InnT.Text.Length > 10)
		{
			InnT.Text = Strings.Mid(InnT.Text, 1, 10);
		}
	}
}
