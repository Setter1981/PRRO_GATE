using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormOperators : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиОператораToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиОператораToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("СкопіюватиНомерСертифікатаToolStripMenuItem")]
	private ToolStripMenuItem _СкопіюватиНомерСертифікатаToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ВидалитиСертифікатToolStripMenuItem")]
	private ToolStripMenuItem _ВидалитиСертифікатToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("DefaultOpToolStripMenuItem")]
	private ToolStripMenuItem _DefaultOpToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ВидалитиОператоравідкладенийКлючToolStripMenuItem")]
	private ToolStripMenuItem _ВидалитиОператоравідкладенийКлючToolStripMenuItem;

	private int nr;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			DataGridViewCellEventHandler value2 = DG_CellDoubleClick;
			DataGridViewCellEventHandler value3 = DG_CellContentClick;
			DataGridViewCellEventHandler value4 = DG_CellClick;
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= value2;
				dG.CellContentClick -= value3;
				dG.CellClick -= value4;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += value2;
				dG.CellContentClick += value3;
				dG.CellClick += value4;
			}
		}
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДодатиОператораToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиОператораToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ДодатиОператораToolStripMenuItem_Click;
			ToolStripMenuItem додатиОператораToolStripMenuItem = _ДодатиОператораToolStripMenuItem;
			if (додатиОператораToolStripMenuItem != null)
			{
				додатиОператораToolStripMenuItem.Click -= value2;
			}
			_ДодатиОператораToolStripMenuItem = value;
			додатиОператораToolStripMenuItem = _ДодатиОператораToolStripMenuItem;
			if (додатиОператораToolStripMenuItem != null)
			{
				додатиОператораToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem СкопіюватиНомерСертифікатаToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _СкопіюватиНомерСертифікатаToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = СкопіюватиНомерСертифікатаToolStripMenuItem_Click;
			ToolStripMenuItem скопіюватиНомерСертифікатаToolStripMenuItem = _СкопіюватиНомерСертифікатаToolStripMenuItem;
			if (скопіюватиНомерСертифікатаToolStripMenuItem != null)
			{
				скопіюватиНомерСертифікатаToolStripMenuItem.Click -= value2;
			}
			_СкопіюватиНомерСертифікатаToolStripMenuItem = value;
			скопіюватиНомерСертифікатаToolStripMenuItem = _СкопіюватиНомерСертифікатаToolStripMenuItem;
			if (скопіюватиНомерСертифікатаToolStripMenuItem != null)
			{
				скопіюватиНомерСертифікатаToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЗакритиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЗакритиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click -= value2;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewTextBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewTextBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column6")]
	internal virtual DataGridViewTextBoxColumn Column6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column7")]
	internal virtual DataGridViewTextBoxColumn Column7
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column8")]
	internal virtual DataGridViewTextBoxColumn Column8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem3")]
	internal virtual ToolStripSeparator ToolStripMenuItem3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ВидалитиСертифікатToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВидалитиСертифікатToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ВидалитиСертифікатToolStripMenuItem_Click;
			ToolStripMenuItem видалитиСертифікатToolStripMenuItem = _ВидалитиСертифікатToolStripMenuItem;
			if (видалитиСертифікатToolStripMenuItem != null)
			{
				видалитиСертифікатToolStripMenuItem.Click -= value2;
			}
			_ВидалитиСертифікатToolStripMenuItem = value;
			видалитиСертифікатToolStripMenuItem = _ВидалитиСертифікатToolStripMenuItem;
			if (видалитиСертифікатToolStripMenuItem != null)
			{
				видалитиСертифікатToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem DefaultOpToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _DefaultOpToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = DefaultOpToolStripMenuItem_Click;
			ToolStripMenuItem defaultOpToolStripMenuItem = _DefaultOpToolStripMenuItem;
			if (defaultOpToolStripMenuItem != null)
			{
				defaultOpToolStripMenuItem.Click -= value2;
			}
			_DefaultOpToolStripMenuItem = value;
			defaultOpToolStripMenuItem = _DefaultOpToolStripMenuItem;
			if (defaultOpToolStripMenuItem != null)
			{
				defaultOpToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem ВидалитиОператоравідкладенийКлючToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ВидалитиОператоравідкладенийКлючToolStripMenuItem_Click;
			ToolStripMenuItem видалитиОператоравідкладенийКлючToolStripMenuItem = _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
			if (видалитиОператоравідкладенийКлючToolStripMenuItem != null)
			{
				видалитиОператоравідкладенийКлючToolStripMenuItem.Click -= value2;
			}
			_ВидалитиОператоравідкладенийКлючToolStripMenuItem = value;
			видалитиОператоравідкладенийКлючToolStripMenuItem = _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
			if (видалитиОператоравідкладенийКлючToolStripMenuItem != null)
			{
				видалитиОператоравідкладенийКлючToolStripMenuItem.Click += value2;
			}
		}
	}

	public FormOperators()
	{
		base.Load += FormOperators_Load;
		nr = 0;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormOperators));
		this.DG = new System.Windows.Forms.DataGridView();
		this.Column1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column4 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column5 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column6 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column7 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column8 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.MenuStrip1 = new System.Windows.Forms.MenuStrip();
		this.МенюToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.СкопіюватиНомерСертифікатаToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem3 = new System.Windows.Forms.ToolStripSeparator();
		this.ВидалитиОператоравідкладенийКлючToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ВидалитиСертифікатToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem1 = new System.Windows.Forms.ToolStripSeparator();
		this.ДодатиОператораToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.DefaultOpToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem2 = new System.Windows.Forms.ToolStripSeparator();
		this.ЗакритиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		((System.ComponentModel.ISupportInitialize)this.DG).BeginInit();
		this.MenuStrip1.SuspendLayout();
		base.SuspendLayout();
		this.DG.AllowUserToAddRows = false;
		this.DG.AllowUserToDeleteRows = false;
		this.DG.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.DG.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG.Columns.AddRange(this.Column1, this.Column2, this.Column3, this.Column4, this.Column5, this.Column6, this.Column7, this.Column8);
		this.DG.Location = new System.Drawing.Point(3, 31);
		this.DG.Name = "DG";
		this.DG.ReadOnly = true;
		this.DG.RowHeadersWidth = 51;
		this.DG.RowTemplate.Height = 24;
		this.DG.Size = new System.Drawing.Size(1103, 416);
		this.DG.TabIndex = 0;
		this.Column1.HeaderText = "№";
		this.Column1.MinimumWidth = 6;
		this.Column1.Name = "Column1";
		this.Column1.ReadOnly = true;
		this.Column1.Width = 50;
		this.Column2.HeaderText = "ПІБ";
		this.Column2.MinimumWidth = 6;
		this.Column2.Name = "Column2";
		this.Column2.ReadOnly = true;
		this.Column2.Width = 180;
		this.Column3.HeaderText = "Шлях до файлу ключа";
		this.Column3.MinimumWidth = 6;
		this.Column3.Name = "Column3";
		this.Column3.ReadOnly = true;
		this.Column3.Width = 174;
		this.Column4.HeaderText = "Пароль";
		this.Column4.MinimumWidth = 6;
		this.Column4.Name = "Column4";
		this.Column4.ReadOnly = true;
		this.Column4.Width = 75;
		this.Column5.HeaderText = "ІНН";
		this.Column5.MinimumWidth = 6;
		this.Column5.Name = "Column5";
		this.Column5.ReadOnly = true;
		this.Column5.Width = 90;
		this.Column6.HeaderText = "Сертифікат";
		this.Column6.MinimumWidth = 6;
		this.Column6.Name = "Column6";
		this.Column6.ReadOnly = true;
		this.Column6.Width = 125;
		this.Column7.HeaderText = "Початок дії";
		this.Column7.MinimumWidth = 6;
		this.Column7.Name = "Column7";
		this.Column7.ReadOnly = true;
		this.Column7.Width = 117;
		this.Column8.HeaderText = "Кінець дії";
		this.Column8.MinimumWidth = 6;
		this.Column8.Name = "Column8";
		this.Column8.ReadOnly = true;
		this.Column8.Width = 108;
		this.MenuStrip1.ImageScalingSize = new System.Drawing.Size(20, 20);
		this.MenuStrip1.Items.AddRange(new System.Windows.Forms.ToolStripItem[1] { this.МенюToolStripMenuItem });
		this.MenuStrip1.Location = new System.Drawing.Point(0, 0);
		this.MenuStrip1.Name = "MenuStrip1";
		this.MenuStrip1.Size = new System.Drawing.Size(1111, 28);
		this.MenuStrip1.TabIndex = 1;
		this.MenuStrip1.Text = "MenuStrip1";
		this.МенюToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[9] { this.СкопіюватиНомерСертифікатаToolStripMenuItem, this.ToolStripMenuItem3, this.ВидалитиОператоравідкладенийКлючToolStripMenuItem, this.ВидалитиСертифікатToolStripMenuItem, this.ToolStripMenuItem1, this.ДодатиОператораToolStripMenuItem, this.DefaultOpToolStripMenuItem, this.ToolStripMenuItem2, this.ЗакритиToolStripMenuItem });
		this.МенюToolStripMenuItem.Name = "МенюToolStripMenuItem";
		this.МенюToolStripMenuItem.Size = new System.Drawing.Size(65, 24);
		this.МенюToolStripMenuItem.Text = "Меню";
		this.СкопіюватиНомерСертифікатаToolStripMenuItem.Name = "СкопіюватиНомерСертифікатаToolStripMenuItem";
		this.СкопіюватиНомерСертифікатаToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.СкопіюватиНомерСертифікатаToolStripMenuItem.Text = "Скопіювати номер сертифіката ";
		this.ToolStripMenuItem3.Name = "ToolStripMenuItem3";
		this.ToolStripMenuItem3.Size = new System.Drawing.Size(382, 6);
		this.ВидалитиОператоравідкладенийКлючToolStripMenuItem.Name = "ВидалитиОператоравідкладенийКлючToolStripMenuItem";
		this.ВидалитиОператоравідкладенийКлючToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.ВидалитиОператоравідкладенийКлючToolStripMenuItem.Text = "Видалити оператора (відкладений ключ)...";
		this.ВидалитиСертифікатToolStripMenuItem.Name = "ВидалитиСертифікатToolStripMenuItem";
		this.ВидалитиСертифікатToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.ВидалитиСертифікатToolStripMenuItem.Text = "Видалити сертифікат";
		this.ToolStripMenuItem1.Name = "ToolStripMenuItem1";
		this.ToolStripMenuItem1.Size = new System.Drawing.Size(382, 6);
		this.ДодатиОператораToolStripMenuItem.Name = "ДодатиОператораToolStripMenuItem";
		this.ДодатиОператораToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.ДодатиОператораToolStripMenuItem.Text = "Додати оператора...";
		this.DefaultOpToolStripMenuItem.Name = "DefaultOpToolStripMenuItem";
		this.DefaultOpToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.DefaultOpToolStripMenuItem.Text = "Оператор за замовчуванням";
		this.ToolStripMenuItem2.Name = "ToolStripMenuItem2";
		this.ToolStripMenuItem2.Size = new System.Drawing.Size(382, 6);
		this.ЗакритиToolStripMenuItem.Name = "ЗакритиToolStripMenuItem";
		this.ЗакритиToolStripMenuItem.Size = new System.Drawing.Size(385, 26);
		this.ЗакритиToolStripMenuItem.Text = "Закрити";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1111, 453);
		base.Controls.Add(this.DG);
		base.Controls.Add(this.MenuStrip1);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		this.MinimumSize = new System.Drawing.Size(900, 300);
		base.Name = "FormOperators";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Оператори";
		((System.ComponentModel.ISupportInitialize)this.DG).EndInit();
		this.MenuStrip1.ResumeLayout(false);
		this.MenuStrip1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormOperators_Load(object sender, EventArgs e)
	{
		ВидалитиСертифікатToolStripMenuItem.Visible = false;
		LoadOperators();
		Application.DoEvents();
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				OperatorsAll operatorsAll = new OperatorsAll();
				if (operatorsAll.Operators < 1)
				{
					nr = -1;
				}
				Coding coding = new Coding();
				int operators = operatorsAll.Operators;
				for (int i = 1; i <= operators; i++)
				{
					DG.RowCount++;
					DG[0, DG.RowCount - 1].Value = operatorsAll.get_Seller(0, i);
					DG[1, DG.RowCount - 1].Value = operatorsAll.get_Seller(1, i);
					DG[2, DG.RowCount - 1].Value = operatorsAll.get_Seller(2, i);
					DG[3, DG.RowCount - 1].Value = "*********";
					DG[4, DG.RowCount - 1].Value = operatorsAll.get_Seller(4, i);
					string section = operatorsAll.get_Seller(4, i);
					IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini");
					string value = DateTime.Now.Day.ToString();
					TypErrStrCert typErrStrCert = All.SF.Cert(operatorsAll.get_Seller(2, i), coding.DeCod(operatorsAll.get_Seller(3, i)));
					if (typErrStrCert.errCode == 0)
					{
						DG[5, DG.RowCount - 1].Value = typErrStrCert.ReturnStr;
						DG[6, DG.RowCount - 1].Value = typErrStrCert.ReturnStart;
						DG[7, DG.RowCount - 1].Value = typErrStrCert.ReturnEnd;
						iniHGB.WriteString(section, "StartKey", typErrStrCert.ReturnStart);
						iniHGB.WriteString(section, "EndKey", typErrStrCert.ReturnEnd);
						iniHGB.WriteString(section, "Issuer", typErrStrCert.ReturnIssuer);
						iniHGB.WriteString(section, "Serial", typErrStrCert.ReturnSerial);
						iniHGB.WriteString(section, "Updated", value);
					}
					else
					{
						iniHGB.WriteString(section, "Updated", "0");
						All.Lg.SaveTextToLog("Оператор: " + operatorsAll.get_Seller(1, i), "Ошибка: " + typErrStrCert.errCode, "Описание ошибки: " + typErrStrCert.errStr);
					}
				}
				if (operatorsAll.Operators > 1)
				{
					DG.Rows[0].DefaultCellStyle.BackColor = Color.FromArgb(227, 255, 255);
					DefaultOpToolStripMenuItem.Enabled = false;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Форма Операторы", "Ошибка заполнения таблицы с операторами", "Критическая ошибка");
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ДодатиОператораToolStripMenuItem_Click(object sender, EventArgs e)
	{
		new FormOperator(NewOperator: true).ShowDialog();
		LoadOperators();
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		OperatorsAll operatorsAll = new OperatorsAll();
		int num = checked(e.RowIndex + 1);
		new FormOperator(NewOperator: false, operatorsAll.get_Seller(0, num), operatorsAll.get_Seller(1, num), operatorsAll.get_Seller(4, num), operatorsAll.get_Seller(3, num), operatorsAll.get_Seller(2, num)).ShowDialog();
		LoadOperators();
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			nr = e.RowIndex;
		}
		if (Conversions.ToInteger(DG[0, nr].Value.ToString()) > 1)
		{
			DefaultOpToolStripMenuItem.Enabled = true;
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void СкопіюватиНомерСертифікатаToolStripMenuItem_Click(object sender, EventArgs e)
	{
		try
		{
			Clipboard.SetText(DG[5, nr].Value.ToString());
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Clipboard.SetText("");
			ProjectData.ClearProjectError();
		}
	}

	private void DG_CellClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			nr = e.RowIndex;
		}
		if (Conversions.ToInteger(DG[0, nr].Value.ToString()) > 1)
		{
			DefaultOpToolStripMenuItem.Enabled = true;
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ВидалитиСертифікатToolStripMenuItem_Click(object sender, EventArgs e)
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		try
		{
			string tINN = DG[4, nr].Value.ToString();
			typErr = All.SF.CertDel(tINN);
			if (typErr.errCode == 0)
			{
				LoadOperators();
				Application.DoEvents();
				Interaction.MsgBox("Сертифікат видалено!", MsgBoxStyle.OkOnly, "Видалення сертифікату");
			}
			else
			{
				Interaction.MsgBox("Помилка видалення сертифіката: " + typErr.errStr, MsgBoxStyle.Critical, "Увага!");
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox("Помилка видалення сертифіката", MsgBoxStyle.Critical, "Увага!");
			ProjectData.ClearProjectError();
		}
	}

	private void DefaultOpToolStripMenuItem_Click(object sender, EventArgs e)
	{
		int num = Conversions.ToInteger(DG[0, nr].Value.ToString());
		if (num > 1)
		{
			try
			{
				new OperatorsAll().OperatorDefault(num);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Виникла помилка при установці оператора за замовчуванням: ", ex2.Message);
				ProjectData.ClearProjectError();
			}
			LoadOperators();
			Application.DoEvents();
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ВидалитиОператоравідкладенийКлючToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (All.l.OfflineTrue())
		{
			Interaction.MsgBox("Для видалення оператора необхідно вийти з офлайн режиму", MsgBoxStyle.Exclamation, "Увага");
			return;
		}
		if (Conversions.ToInteger(All.l.ReturnOpenShift().ReturnStr) > 0)
		{
			Interaction.MsgBox("Для видалення оператора необхідно закрити зміну", MsgBoxStyle.Exclamation, "Увага");
			return;
		}
		int index = DG.CurrentRow.Index;
		if (Operators.CompareString(DG[0, index].Value.ToString(), "1", TextCompare: false) == 0)
		{
			Interaction.MsgBox("Першого оператора видаляти не можна", MsgBoxStyle.Exclamation, "Увага");
		}
		else if (Interaction.MsgBox("Ви дійсно хочете видалити вибраного оператора (відкладений ключ)?", MsgBoxStyle.Exclamation | MsgBoxStyle.YesNo, "Увага") != MsgBoxResult.No)
		{
			string text = DG[4, index].Value.ToString();
			UpdateKeys updateKeys = new UpdateKeys();
			updateKeys.DelOp(text.ToString());
			updateKeys.DelKey(text.ToString());
			LoadOperators();
		}
	}
}
